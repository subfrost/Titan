use borsh::{BorshDeserialize, BorshSerialize};

pub trait Entry: Sized + BorshDeserialize + BorshSerialize {
    fn load(value: Vec<u8>) -> Self {
        BorshDeserialize::deserialize(&mut &value[..]).unwrap()
    }

    fn store(self) -> Vec<u8> {
        let mut serialized = Vec::new();
        self.serialize(&mut serialized).unwrap();
        serialized
    }
}
