class ArrearsService {
    static final ArrearsService INSTANCE = new ArrearsService();

    int positiveTotal(int accountId) {
        int total = 0;
        BillRow[] rows;
        /* @normalizer-ir
           rows = from bill as b
               where b.account_id == accountId and b.status == "OPEN"
               yield { accountId: b.account_id, amount: b.amount, status: b.status }
        */
        /* @normalizer-ir
           total += sum from rows as row
               where row.amount > 10
               yield row.amount
        */
        return total;
    }
}

class BillRow {
    int accountId;
    int amount;
    String status;
}
