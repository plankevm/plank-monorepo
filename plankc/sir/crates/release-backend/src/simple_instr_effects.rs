enum Region {
    TransientStorage,
    PersistentStorage,
    // Balances, Extcodehashes
    AccountState,
    Memory,
    Returndata,
    LogOrder,
}

enum ReadWrite {
    Read,
    Write,
}

struct Effect {
    region: Region,
    read_write: ReadWrite,
}

impl Effect {
    const fn new(region: Region, read_write: ReadWrite) -> Self {
        Self { region, read_write }
    }
}
