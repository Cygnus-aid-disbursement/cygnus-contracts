use soroban_sdk::{contracttype, Address, BytesN, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    pub description_hash: BytesN<32>,
    pub amount: i128,
    pub due_by: u64,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    Pending,
    Claimed,
    Approved,
    Rejected,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneState {
    pub status: MilestoneStatus,
    // Evidence and rejection references are hashes only. The documents behind
    // them never touch the chain.
    pub evidence_hash: Option<BytesN<32>>,
    pub reason_hash: Option<BytesN<32>>,
    pub claimed_at: u64,
    // True when the claim landed after due_by. A late claim is flagged, never
    // blocked, because a field office that misses a date has still done the work.
    pub claimed_late: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApproverConfig {
    Single(Address),
    Panel(Vec<Address>, u32),
    Oracle(Address),
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgrammeStatus {
    Active,
    Closed,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisbursementBatch {
    pub index: u32,
    // A batch commits to individual payouts without publishing them. Only the
    // root, the total and the count are ever on chain.
    pub root: BytesN<32>,
    pub total: i128,
    pub count: u32,
    pub disbursed_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Programme {
    pub id: u64,
    pub sponsor: Address,
    pub implementer: Address,
    pub asset: Address,
    pub total: i128,
    pub funded: i128,
    pub released: i128,
    pub disbursed: i128,
    pub approver: ApproverConfig,
    pub metadata_hash: BytesN<32>,
    pub milestones: Vec<Milestone>,
    pub milestone_states: Vec<MilestoneState>,
    pub status: ProgrammeStatus,
    // The programme ends at the latest milestone due_by. refund_remainder is
    // legal only after ledger time passes this point.
    pub end_by: u64,
    pub batch_count: u32,
    pub created_at: u64,
}
