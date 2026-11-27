class ArrearsService {
    static final ArrearsService INSTANCE = new ArrearsService();

    int firstPayableAmount(int accountId, int today) {
        BillRow[] rows;
        /* @normalizer-ir
           rows = from bill as b
               where b.account_id == accountId and b.status == "OPEN"
               order by b.priority, b.due_date, b.id
               yield { id: b.id, accountId: b.account_id, dueDate: b.due_date,
                       amount: b.amount, priority: b.priority }
        */
        BillRow match;
        /* @normalizer-ir
           match = first from rows as row
               where row.dueDate <= today and row.amount > 0
               yield row else null
        */
        if (match != null) return match.amount;
        return -7;
    }
}

class BillRow {
    int id;
    int accountId;
    int dueDate;
    int amount;
    int priority;
}
