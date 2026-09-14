use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    ProgrammeNotFound = 1,
    AlreadyClosed = 2,
    MilestoneIndexOutOfRange = 3,
    MilestoneNotClaimable = 4,
    MilestoneNotClaimed = 5,
    MilestoneAlreadyResolved = 6,
    InsufficientFunding = 7,
    InsufficientReleased = 8,
    NotEnded = 9,
    InvalidAmount = 10,
    InvalidBatch = 11,
    // Panel and Oracle approvers are declared in ApproverConfig but not built at
    // the 65% line. They return this so later work is purely additive.
    ApproverKindUnimplemented = 12,
    NothingToRefund = 13,
    BatchIndexOutOfRange = 14,
}
