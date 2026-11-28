class AccountJoinRow { int id; }
class BillJoinRow { int account_id; String status; }
class JoinService {
    static int openBillCount(int[] options, int accountId) {
        AccountJoinRow[] accounts;
        /* @normalizer-ir
           accounts = from account as row yield { id: row.id }
        */
        BillJoinRow[] bills;
        /* @normalizer-ir
           bills = from bill as row yield { account_id: row.account_id, status: row.status }
        */
        int count;
        /* @normalizer-ir
           count = count from accounts as a
               join bills as b on a.id == b.account_id
               where a.id == accountId and b.status == "OPEN"
        */
        int selected;
        /* @normalizer-ir selected = choose from options else count */
        return selected;
    }
}
