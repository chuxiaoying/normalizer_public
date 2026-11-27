class AccountRow { int accountId; }
class BillReport {
    static long countAccounts() {
        AccountRow[] pending;
        /* @normalizer-ir pending = from pending_bill as p yield {accountId: p.account_id} */
        AccountRow[] posted;
        /* @normalizer-ir posted = from posted_bill as q yield {accountId: q.account_id} */
        AccountRow[] alternate; /* @normalizer-ir alternate = from pending_bill as q yield {accountId: q.alternate_account_id} */ long count;
        /* @normalizer-ir count = count distinct union pending.accountId, alternate.accountId, posted.accountId */
        return count;
    }
}
