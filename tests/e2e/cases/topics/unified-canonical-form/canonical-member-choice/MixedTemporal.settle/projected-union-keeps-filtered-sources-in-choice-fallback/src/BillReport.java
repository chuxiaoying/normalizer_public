class AccountRow { int accountId; }
class BillReport {
    static long countAccounts(int[] options) {
        AccountRow[] pending;
        /* @normalizer-ir pending = from pending_bill as p yield {accountId: p.account_id} */
        AccountRow[] posted;
        /* @normalizer-ir posted = from posted_bill as q where q.account_id > 0 yield {accountId: q.account_id} */
        long count;
        /* @normalizer-ir count = count distinct union pending.accountId, posted.accountId */
        long selected;
        /* @normalizer-ir selected = choose from options else count */
        return selected;
    }
}
