extern crate base64;
extern crate openssl;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use base64::{decode_config, encode_config, URL_SAFE};
use openssl::hash::MessageDigest;
use serde::{Serialize, Deserialize};
pub use error::Error;
pub use header::DefaultHeader;
pub use header::Algorithm;
pub use claims::Claims;
pub use claims::Registered;

pub mod error;
pub mod header;
pub mod claims;
mod crypt;

#[derive(Debug, Default)]
pub struct Token<H, C>
    where H: Component, C: Component {
    raw: Option<String>,
    pub header: H,
    pub claims: C,
}

pub trait Header {
    fn alg(&self) -> &header::Algorithm;
}

pub trait Component: Sized {
    fn from_base64(raw: &str) -> Result<Self, Error>;
    fn to_base64(&self) -> Result<String, Error>;
}

impl<T> Component for T
    where T: Serialize + Deserialize + Sized {

    /// Parse from a string.
    fn from_base64(raw: &str) -> Result<T, Error> {
        let data = try!(decode_config(raw, URL_SAFE));
        let s = try!(String::from_utf8(data));
        Ok(try!(serde_json::from_str(&*s)))
    }

    /// Encode to a string.
    fn to_base64(&self) -> Result<String, Error> {
        let s = try!(serde_json::to_string(&self));
        let enc = encode_config((&*s).as_bytes(), URL_SAFE);
        Ok(enc)
    }
}

impl<H, C> Token<H, C>
    where H: Component + Header, C: Component {
    pub fn new(header: H, claims: C) -> Token<H, C> {
        Token {
            raw: None,
            header: header,
            claims: claims,
        }
    }

    /// Parse a token from a string.
    pub fn parse(raw: &str) -> Result<Token<H, C>, Error> {
        let pieces: Vec<_> = raw.split('.').collect();

        Ok(Token {
            raw: Some(raw.into()),
            header: try!(Component::from_base64(pieces[0])),
            claims: try!(Component::from_base64(pieces[1])),
        })
    }

    /// Verify a from_base64 token with a key and the token's specific algorithm
    pub fn verify(&self, key: &[u8]) -> bool {
        match self.header.alg() {
            &Algorithm::HS256 => self.verify_hmac(key, MessageDigest::sha256()),
            &Algorithm::HS384 => self.verify_hmac(key, MessageDigest::sha384()),
            &Algorithm::HS512 => self.verify_hmac(key, MessageDigest::sha512()),
            &Algorithm::RS256 => self.verify_rsa(key, MessageDigest::sha256()),
            &Algorithm::RS384 => self.verify_rsa(key, MessageDigest::sha384()),
            &Algorithm::RS512 => self.verify_rsa(key, MessageDigest::sha512()),
        }
    }

    fn verify_hmac(&self, key: &[u8], digest: MessageDigest) -> bool {
        let raw = match self.raw {
            Some(ref s) => s,
            None => return false,
        };

        let pieces: Vec<_> = raw.rsplitn(2, '.').collect();
        let sig = pieces[0];
        let data = pieces[1];

        crypt::verify(sig, data, key, digest)
    }

    fn verify_rsa(&self, key: &[u8], digest: MessageDigest) -> bool {
        let raw = match self.raw {
            Some(ref s) => s,
            None => return false,
        };

        let pieces: Vec<_> = raw.rsplitn(2, '.').collect();
        let sig = pieces[0];
        let data = pieces[1];

        crypt::verify_rsa(sig, data, key, digest)
    }

    /// Generate the signed token from a key and the specific algorithm
    pub fn signed(&self, key: &[u8]) -> Result<String, Error> {
        match self.header.alg() {
            &Algorithm::HS256 => self.signed_hmac(key, MessageDigest::sha256()),
            &Algorithm::HS384 => self.signed_hmac(key, MessageDigest::sha384()),
            &Algorithm::HS512 => self.signed_hmac(key, MessageDigest::sha512()),
            &Algorithm::RS256 => self.signed_rsa(key, MessageDigest::sha256()),
            &Algorithm::RS384 => self.signed_rsa(key, MessageDigest::sha384()),
            &Algorithm::RS512 => self.signed_rsa(key, MessageDigest::sha512()),
        }
    }

    fn signed_hmac(&self, key: &[u8], digest: MessageDigest) -> Result<String, Error> {
        let header = try!(Component::to_base64(&self.header));
        let claims = try!(self.claims.to_base64());
        let data = format!("{}.{}", header, claims);

        let sig = crypt::sign(&*data, key, digest);
        Ok(format!("{}.{}", data, sig))
    }

    fn signed_rsa(&self, key: &[u8], digest: MessageDigest) -> Result<String, Error> {
        let header = try!(Component::to_base64(&self.header));
        let claims = try!(self.claims.to_base64());
        let data = format!("{}.{}", header, claims);

        let sig = crypt::sign_rsa(&*data, key, digest);
        Ok(format!("{}.{}", data, sig))
    }
}

impl<H, C> PartialEq for Token<H, C>
    where H: Component + PartialEq, C: Component + PartialEq{
    fn eq(&self, other: &Token<H, C>) -> bool {
        self.header == other.header &&
        self.claims == other.claims
    }
}

#[cfg(test)]
mod tests {
    use crypt::{
        sign,
        sign_rsa,
        verify,
        verify_rsa
    };
    use Claims;
    use Token;
    use header::Algorithm::{HS256,RS512};
    use header::DefaultHeader;
    use std::io::{Error, Read};
    use std::fs::File;
    use openssl::hash::MessageDigest;

    #[derive(Default, Debug, Serialize, Deserialize, PartialEq)]
    struct EmptyClaim { }

    #[test]
    pub fn sign_data() {
        let header = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let claims = "eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWV9";
        let real_sig = "TJVA95OrM7E2cBab30RMHrHDcEfxjoYZgeFONFh7HgQ=";
        let data = format!("{}.{}", header, claims);

        let sig = sign(&*data, "secret".as_bytes(), MessageDigest::sha256());

        assert_eq!(sig, real_sig);
    }

    #[test]
    pub fn sign_data_rsa() {
        let header = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9";
        let claims = "eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWV9";
        let real_sig = "nXdpIkFQYZXZ0VlJjHmAc5_aewHCCJpT5jP1fpexUCF_9m3NxlC7uYNXAl6NKno520oh9wVT4VV_vmPeEin7BnnoIJNPcImWcUzkYpLTrDBntiF9HCuqFaniuEVzlf8dVlRJgo8QxhmUZEjyDFjPZXZxPlPV1LD6hrtItxMKZbh1qoNY3OL7Mwo-WuSRQ0mmKj-_y3weAmx_9EaTLY639uD8-o5iZxIIf85U4e55Wdp-C9FJ4RxyHpjgoG8p87IbChfleSdWcZL3NZuxjRCHVWgS1uYG0I-LqBWpWyXnJ1zk6-w4tfxOYpZFMOIyq4tY2mxJQ78Kvcu8bTO7UdI7iA==";
        let data = format!("{}.{}", header, claims);

        let key = load_key("./examples/privateKey.pem").unwrap();

        let sig = sign_rsa(&*data, key.as_bytes(), MessageDigest::sha256());

        assert_eq!(sig.trim(), real_sig);
    }

    #[test]
    pub fn verify_data() {
        let header = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let claims = "eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWV9";
        let target = "TJVA95OrM7E2cBab30RMHrHDcEfxjoYZgeFONFh7HgQ";
        let data = format!("{}.{}", header, claims);

        assert!(verify(target, &*data, "secret".as_bytes(), MessageDigest::sha256()));
    }

    #[test]
    pub fn verify_data_rsa() {
        let header = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9";
        let claims = "eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWV9";
        let real_sig = "nXdpIkFQYZXZ0VlJjHmAc5_aewHCCJpT5jP1fpexUCF_9m3NxlC7uYNXAl6NKno520oh9wVT4VV_vmPeEin7BnnoIJNPcImWcUzkYpLTrDBntiF9HCuqFaniuEVzlf8dVlRJgo8QxhmUZEjyDFjPZXZxPlPV1LD6hrtItxMKZbh1qoNY3OL7Mwo-WuSRQ0mmKj-_y3weAmx_9EaTLY639uD8-o5iZxIIf85U4e55Wdp-C9FJ4RxyHpjgoG8p87IbChfleSdWcZL3NZuxjRCHVWgS1uYG0I-LqBWpWyXnJ1zk6-w4tfxOYpZFMOIyq4tY2mxJQ78Kvcu8bTO7UdI7iA";
        let data = format!("{}.{}", header, claims);

        let key = load_key("./examples/publicKey.pub").unwrap();
        assert!(verify_rsa(&real_sig, &*data, key.as_bytes(), MessageDigest::sha256()));
    }

    #[test]
    pub fn raw_data() {
        let raw = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiYWRtaW4iOnRydWV9.TJVA95OrM7E2cBab30RMHrHDcEfxjoYZgeFONFh7HgQ";
        let token = Token::<DefaultHeader, Claims<EmptyClaim>>::parse(raw).unwrap();

        {
            assert_eq!(token.header.alg, HS256);
        }
        assert!(token.verify("secret".as_bytes()));
    }

    #[test]
    pub fn roundtrip() {
        let token: Token<DefaultHeader, Claims<EmptyClaim>> = Default::default();
        let key = "secret".as_bytes();
        let raw = token.signed(key).unwrap();
        let same = Token::parse(&*raw).unwrap();

        assert_eq!(token, same);
        assert!(same.verify(key));
    }

    #[test]
    pub fn roundtrip_rsa() {
        let token: Token<DefaultHeader, Claims<EmptyClaim>> = Token {
            header: DefaultHeader {
                alg: RS512,
                ..Default::default()
            },
            ..Default::default()
        };
        let private_key = load_key("./examples/privateKey.pem").unwrap();
        let raw = token.signed(private_key.as_bytes()).unwrap();
        let same = Token::parse(&*raw).unwrap();

        assert_eq!(token, same);
        let public_key = load_key("./examples/publicKey.pub").unwrap();
        assert!(same.verify(public_key.as_bytes()));
    }

    fn load_key(keypath: &str) -> Result<String, Error> {
        let mut key_file = try!(File::open(keypath));
        let mut key = String::new();
        try!(key_file.read_to_string(&mut key));
        Ok(key)
    }
}
