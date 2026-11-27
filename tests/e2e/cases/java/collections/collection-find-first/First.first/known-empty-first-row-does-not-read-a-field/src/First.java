import normalizer.intrinsics.N;

class First {
    static int first(int accountId) {
        BillRow[] rows;
        /* @normalizer-ir
           rows = from bill as b
               where b.account_id == accountId
               yield { amount: b.amount }
        */
        @N.Nullable BillRow match;
        /* @normalizer-ir
           match = first from rows as row
               where row.amount > 0 and false
               order by row.amount
               yield row else null
        */
        if (match != null) return match.amount;
        return 0;
    }
}

class BillRow {
    int amount;
}
