pub trait Hasher {
    type Output;
    fn new() -> Self;
    fn update(&mut self, data: &[u8]);
    fn finalize(self) -> Self::Output;
}
