use anyhow::Error;

pub enum Flag {
    NullButHadUnparseableValue(Error, serde_json::Value),
}

pub struct DeserializerConditions {
    flags: Vec<Flag>,
}

impl DeserializerConditions {
    pub fn add_flag(mut self, flag: Flag) -> Self {
        self.flags.push(flag);
        self
    }

    pub fn new() -> Self {
        Self { flags: Vec::new() }
    }

    fn score(&self) -> usize {
        self.flags.len()
    }
}

impl Eq for DeserializerConditions {}

impl PartialEq for DeserializerConditions {
    fn eq(&self, other: &Self) -> bool {
        self.score() == other.score()
    }
}

impl PartialOrd for DeserializerConditions {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DeserializerConditions {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score().cmp(&other.score())
    }
}
