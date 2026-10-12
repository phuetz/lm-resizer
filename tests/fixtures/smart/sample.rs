use std::fmt;
/// Limite publique.
pub const LIMIT: usize = 999;
pub struct Counter {
    pub value: usize,
}
impl Counter {
    pub fn add(&mut self, amount: usize) -> usize { // Incrémente le compteur.
        self.value += amount; self.value
    }
}
pub trait Named {
    fn name(&self) -> &str;
}
pub type Counts = Vec<usize>;
pub async fn fetch<T: Send>(value: T) -> T { value }
const TEXT: &str = "fn textual_fake() {}";
