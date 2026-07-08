use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct NodeIdentity {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl Drop for NodeIdentity {
    fn drop(&mut self) {
        self.signing_key.zeroize();
    }
}
