use serde::Serialize;
use serde::de::DeserializeOwned;
pub use self::header::Header;
pub use self::header::Algorithm;
pub use self::payload::Payload;
use Result;

mod header;
mod payload;
mod crypt;

/// Main struct representing a JSON Web Token, composed of a header and a set of claims.
#[derive(Debug, Default)]
pub struct Token<H, C> {
    raw: Option<String>,
    pub header: Header<H>,
    pub payload: Payload<C>,
}

/// Provide the ability to parse a token, verify it and sign/serialize it.
impl<H, C> Token<H, C>
    where H: Serialize + DeserializeOwned,
          C: Serialize + DeserializeOwned
{
    pub fn new(header: Header<H>, payload: Payload<C>) -> Token<H, C> {
        Token {
            raw: None,
            header: header,
            payload: payload,
        }
    }

    /// Parse a token from a string.
    pub fn parse(raw: &str) -> Result<Token<H, C>> {
        let pieces: Vec<_> = raw.split('.').collect();

        Ok(Token {
            raw: Some(raw.to_owned()),
            header: Header::from_base64(pieces[0])?,
            payload: Payload::from_base64(pieces[1])?,
        })
    }

    /// Verify a token with a key and the token's specific algorithm.
    pub fn verify(&self, key: &[u8]) -> Result<bool> {
        let raw = match self.raw {
            Some(ref s) => s,
            None => return Ok(false),
        };

        let pieces: Vec<_> = raw.rsplitn(2, '.').collect();
        let sig = pieces[0];
        let data = pieces[1];

        Ok(self.payload.verify() && crypt::verify(sig, data, key, &self.header.alg)?)
    }

    /// Generate the signed token from a key with the specific algorithm as a url-safe, base64
    /// string.
    pub fn sign(&self, key: &[u8]) -> Result<String> {
        let header = self.header.to_base64()?;
        let payload = self.payload.to_base64()?;
        let data = format!("{}.{}", header, payload);

        let sig = crypt::sign(&*data, key, &self.header.alg)?;
        Ok(format!("{}.{}", data, sig))
    }
}

impl<H, C> PartialEq for Token<H, C>
    where H: PartialEq,
          C: PartialEq
{
    fn eq(&self, other: &Token<H, C>) -> bool {
        self.header == other.header && self.payload == other.payload
    }
}

#[cfg(test)]
mod tests {
    use {DefaultPayload, DefaultToken, Header};
    use openssl;
    use std::default::Default;
    use time::{self, Duration, Tm};
    use super::Algorithm::{HS256, RS512};

    #[test]
    pub fn raw_data() {
        let raw = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
                   eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWV9.\
                   TJVA95OrM7E2cBab30RMHrHDcEfxjoYZgeFONFh7HgQ";
        let token = DefaultToken::<()>::parse(raw).unwrap();

        assert_eq!(token.header.alg, HS256);
        assert!(token.verify("secret".as_bytes()).unwrap());
    }

    #[test]
    pub fn roundtrip_hmac() {
        let now = time::now();
        let header: Header<()> = Default::default();
        let payload = DefaultPayload {
            nbf: Some(now.to_timespec().sec as u64),
            exp: Some((now + Duration::minutes(5)).to_timespec().sec as u64),
            ..Default::default()
        };
        let token = DefaultToken::new(header, payload);
        let key = "secret".as_bytes();
        let raw = token.sign(key).unwrap();
        let same = DefaultToken::parse(&*raw).unwrap();

        assert_eq!(token, same);
        assert!(same.verify(key).unwrap());
    }

    #[test]
    pub fn roundtrip_expired() {
        let now = time::now();
        let token = create_for_range(now, now + Duration::minutes(-5));
        let key = "secret".as_bytes();
        let raw = token.sign(key).unwrap();
        let same = DefaultToken::parse(&*raw).unwrap();

        assert_eq!(token, same);
        assert_eq!(false, same.verify(key).unwrap());
    }

    #[test]
    pub fn roundtrip_not_yet_valid() {
        let now = time::now();
        let token = create_for_range(now + Duration::minutes(5), now + Duration::minutes(10));
        let key = "secret".as_bytes();
        let raw = token.sign(key).unwrap();
        let same = DefaultToken::parse(&*raw).unwrap();

        assert_eq!(token, same);
        assert_eq!(false, same.verify(key).unwrap());
    }

    #[test]
    pub fn roundtrip_rsa() {
        let rsa_keypair = openssl::rsa::Rsa::generate(2048).unwrap();
        let header: Header<()> = Header { alg: RS512, ..Default::default() };
        let token = DefaultToken { header: header, ..Default::default() };
        let raw = token.sign(&rsa_keypair.private_key_to_pem().unwrap()).unwrap();
        let same = DefaultToken::parse(&*raw).unwrap();

        assert_eq!(token, same);
        assert!(same.verify(&rsa_keypair.public_key_to_pem().unwrap()).unwrap());
    }

    fn create_for_range(nbf: Tm, exp: Tm) -> DefaultToken<()> {
        let header: Header<()> = Default::default();
        let payload = DefaultPayload {
            nbf: Some(nbf.to_timespec().sec as u64),
            exp: Some(exp.to_timespec().sec as u64),
            ..Default::default()
        };
        DefaultToken::new(header, payload)
    }
}