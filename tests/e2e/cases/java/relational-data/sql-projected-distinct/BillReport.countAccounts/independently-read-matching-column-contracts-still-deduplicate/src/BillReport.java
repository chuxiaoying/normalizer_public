class AccountRow { int accountId; }
class OtherAccountRow { int id; } class BillReport {
    static long countAccounts() {
        AccountRow[] pending;
        /* @normalizer-ir pending = from pending_bill as p yield {accountId: p.account_id} */
        AccountRow[] posted;
        /* @normalizer-ir posted = from posted_bill as q yield {accountId: q.account_id} */
        OtherAccountRow[] again; /* @normalizer-ir again = from pending_bill as c yield {id: c.account_id} */ long count;
        /* @normalizer-ir count = count distinct union pending.accountId, again.id, posted.accountId */
        return count;
    }
}
