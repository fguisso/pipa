use ulid::Ulid;

pub trait IdGen: Send + Sync {
    fn new_ulid(&self) -> Ulid;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UlidGen;

impl IdGen for UlidGen {
    fn new_ulid(&self) -> Ulid {
        Ulid::new()
    }
}
