use plank_values::ValueId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CachedComptimeValue {
    pub value: ValueId,
    pub quota_record: ComptimeQuotaRecord,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) struct ComptimeQuotaRecord {
    pub branches_consumed: u32,
    pub max_eval_branch_quota: u32,
}

pub(crate) const DEFAULT_COMPTIME_BRANCH_QUOTA: u32 = 1000;

#[derive(Debug, Clone)]
pub(crate) struct ComptimeQuota {
    limit: u32,
    spent: u32,
    records: Vec<ComptimeQuotaRecord>,
}

impl Default for ComptimeQuota {
    fn default() -> Self {
        Self { limit: DEFAULT_COMPTIME_BRANCH_QUOTA, spent: 0, records: Vec::new() }
    }
}

impl ComptimeQuota {
    pub(crate) fn raise_limit(&mut self, limit: u32) {
        self.limit = self.limit.max(limit);
        if let Some(record) = self.records.last_mut() {
            record.max_eval_branch_quota = record.max_eval_branch_quota.max(limit);
        }
    }

    pub(crate) fn spend_branch(&mut self) -> bool {
        debug_assert!(self.spent <= self.limit, "comptime quota overspent");
        if self.spent == self.limit {
            return false;
        }
        self.spent += 1;
        if let Some(record) = self.records.last_mut() {
            record.branches_consumed += 1;
        }
        true
    }

    pub(crate) fn limit(&self) -> u32 {
        self.limit
    }

    pub(crate) fn replay_record(&mut self, replayed: ComptimeQuotaRecord) -> bool {
        if self.spent.checked_add(replayed.branches_consumed).is_none_or(|spent| spent > self.limit)
        {
            return false;
        }
        self.spent += replayed.branches_consumed;

        if let Some(record) = self.records.last_mut() {
            record.branches_consumed += replayed.branches_consumed;
        }
        self.raise_limit(replayed.max_eval_branch_quota);
        true
    }

    pub(crate) fn begin_recording(&mut self) {
        self.records.push(ComptimeQuotaRecord::default());
    }

    pub(crate) fn finish_recording(&mut self) -> ComptimeQuotaRecord {
        let record = self.records.pop().expect("comptime quota recording stack underflow");
        if let Some(parent) = self.records.last_mut() {
            parent.branches_consumed += record.branches_consumed;
            parent.max_eval_branch_quota =
                parent.max_eval_branch_quota.max(record.max_eval_branch_quota);
        }
        record
    }

    pub(crate) fn discard_recording(&mut self) {
        self.records.pop().expect("comptime quota recording stack underflow");
    }
}
