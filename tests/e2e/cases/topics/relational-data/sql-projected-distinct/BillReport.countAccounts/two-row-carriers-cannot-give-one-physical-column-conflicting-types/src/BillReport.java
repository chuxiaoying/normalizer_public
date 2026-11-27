class AccountRow { int accountId; }
class TextAccountRow { String accountId; } class BillReport {
    static long countAccounts() {
        AccountRow[] pending;
        /* @normalizer-ir pending = from pending_bill as p yield {accountId: p.account_id} */
        AccountRow[] posted;
        /* @normalizer-ir posted = from posted_bill as q yield {accountId: q.account_id} */
        TextAccountRow[] conflict; /* @normalizer-ir conflict = from pending_bill as c yield {accountId: c.account_id} */ long count;
        /* @normalizer-ir count = count distinct union pending.accountId, posted.accountId */
        return count;
    }
}
