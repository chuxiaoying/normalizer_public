import normalizer.intrinsics.N;
class BillMemoRow {
    int id;
    @N.Nullable String memo;
}
class NullMemoService {
    static boolean firstRowHasNullMemo(int accountId) {
        BillMemoRow[] bills;
        /* @normalizer-ir
           bills = from bill as b
               where b.account_id == accountId
               yield { id: b.id, memo: b.memo }
        */
        @N.Nullable BillMemoRow match;
        /* @normalizer-ir
           match = first from bills as row
               where row.memo IS NOT NULL order by row.id
               yield row else null
        */
        return match != null;
    }
}
