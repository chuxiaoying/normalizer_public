class PositiveCount {
    static int count(int accountId) {
        int count = 0;
        BillRow[] rows;
        /* @normalizer-ir
           rows = from bill as b
               where b.account_id == accountId
               yield { amount: b.amount }
        */
        /* @normalizer-ir
           count += sum from rows as row
               where row.amount >= 10
               yield 1
        */
        return count;
    }
}

class BillRow {
    int amount;
}
