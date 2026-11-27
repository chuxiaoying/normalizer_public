class AccountRow { int accountId; }
class BillReport {
    static long countAccounts() {
        AccountRow[] early;
        /* @normalizer-ir early = from pending_bill as raw yield {accountId: raw.account_id} */
        AccountRow[] posted;
        /* @normalizer-ir posted = from posted_bill as q yield {accountId: q.account_id} */
        long count;
        /* @normalizer-ir count = count distinct union early.accountId, posted.accountId */
        return count;
    }
}
