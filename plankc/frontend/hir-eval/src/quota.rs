use plank_session::SrcLoc;

pub(crate) const DEFAULT_COMPTIME_BRANCH_QUOTA: u32 = 1000;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ComptimeQuota {
    limit: u32,
    spent: u32,
    start_loc: SrcLoc,
}

#[derive(Debug)]
pub(crate) struct QuotaExhaustedError;

impl ComptimeQuota {
    pub(crate) fn root(start_loc: SrcLoc) -> Self {
        Self { limit: DEFAULT_COMPTIME_BRANCH_QUOTA, spent: 0, start_loc }
    }

    pub(crate) fn inherited_for_call(self, call_loc: SrcLoc) -> Self {
        Self { limit: self.limit, spent: 0, start_loc: call_loc }
    }

    pub(crate) fn raise_limit(&mut self, limit: u32) {
        self.limit = self.limit.max(limit);
    }

    pub(crate) fn can_spend(&self, branches: u32) -> bool {
        assert!(self.spent <= self.limit, "comptime quota overspent elsewhere");
        self.spent.checked_add(branches).is_some_and(|spent| spent <= self.limit)
    }

    pub(crate) fn spend(&mut self, branches: u32) -> Result<(), QuotaExhaustedError> {
        if !self.can_spend(branches) {
            return Err(QuotaExhaustedError);
        }
        self.spent = self.spent.checked_add(branches).expect("capacity checked above");
        Ok(())
    }

    pub(crate) fn limit(&self) -> u32 {
        self.limit
    }

    pub(crate) fn spent(&self) -> u32 {
        self.spent
    }

    pub(crate) fn start_loc(&self) -> SrcLoc {
        self.start_loc
    }
}
