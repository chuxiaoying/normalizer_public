class BillGroupRow {
    String category;
    int account_id;
}
class GroupService {
    static int statusGroupCount() {
        BillGroupRow[] bills;
        /* @normalizer-ir
           bills = from bill as row yield { category: row.status, account_id: row.account_id }
        */
        int count;
        /* @normalizer-ir
           count = count groups from bills as b by b.category
        */
        return count;
    }
}
