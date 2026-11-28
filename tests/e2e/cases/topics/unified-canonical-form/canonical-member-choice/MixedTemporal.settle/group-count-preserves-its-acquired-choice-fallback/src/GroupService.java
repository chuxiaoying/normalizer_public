class BillGroupRow {
    String status;
    int account_id;
}
class GroupService {
    static int statusGroupCount(int[] options) {
        BillGroupRow[] bills;
        /* @normalizer-ir
           bills = from bill as row yield { status: row.status, account_id: row.account_id }
        */
        int count;
        /* @normalizer-ir
           count = count groups from bills as b by b.status
        */
        int selected;
        /* @normalizer-ir selected = choose from options else count */
        return selected;
    }
}
