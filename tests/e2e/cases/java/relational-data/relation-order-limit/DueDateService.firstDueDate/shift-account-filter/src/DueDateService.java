class BillOrderRow {
    int accountId;
    int dueDate;
    int id;
}
class DueDateService {
    static int firstDueDate(int accountId) {
        BillOrderRow[] bills;
        /* @normalizer-ir
           bills = from bill as b
               where b.account_id == accountId + 1
               yield { accountId: b.account_id, dueDate: b.due_date, id: b.id }
        */
        int dueDate;
        /* @normalizer-ir
           dueDate = first from bills as row
               order by row.dueDate, row.id
               yield row.dueDate else 0
        */
        return dueDate;
    }
}
