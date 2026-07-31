pub(crate) const DEFAULT_COMPTIME_BRANCH_QUOTA: u32 = 1000;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ComptimeQuota {
    limit: u32,
    spent: u32,
}

impl Default for ComptimeQuota {
    fn default() -> Self {
        Self { limit: DEFAULT_COMPTIME_BRANCH_QUOTA, spent: 0 }
    }
}

pub(crate) struct QuotaExhaustedError;

impl ComptimeQuota {
    pub(crate) fn raise_limit(&mut self, limit: u32) {
        self.limit = self.limit.max(limit);
    }

    pub(crate) fn spend_branch(&mut self) -> Result<(), QuotaExhaustedError> {
        assert!(self.spent <= self.limit, "comptime quota overspent elsewhere");
        if self.spent == self.limit {
            return Err(QuotaExhaustedError);
        }
        self.spent += 1;
        Ok(())
    }

    pub(crate) fn limit(&self) -> u32 {
        self.limit
    }

    pub(crate) fn spent(&self) -> u32 {
        self.spent
    }

    pub(crate) fn replay_cached_call(
        &mut self,
        branches_consumed: u32,
        max_eval_branch_quota_seen: u32,
    ) -> bool {
        if self.spent.checked_add(branches_consumed).is_none_or(|spent| spent > self.limit) {
            return false;
        }
        self.spent += branches_consumed;

        self.raise_limit(max_eval_branch_quota_seen);
        true
    }
}
