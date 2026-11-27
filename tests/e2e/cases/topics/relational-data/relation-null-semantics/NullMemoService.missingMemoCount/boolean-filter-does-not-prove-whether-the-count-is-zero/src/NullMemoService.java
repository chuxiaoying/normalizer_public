import normalizer.intrinsics.N;

class BillMemoRow {
    int accountId;
    @N.Nullable String memo;
}
class NullMemoService {
    static boolean missingMemoCount(int accountId, boolean enabled) {
        BillMemoRow[] bills;
        /* @normalizer-ir
           bills = from bill as b
               where b.account_id == accountId and enabled
               yield { accountId: b.account_id, memo: b.memo }
        */
        int count;
        /* @normalizer-ir
           count = count from bills as row where row.memo IS NULL
        */
        return count == 0;
    }
}
