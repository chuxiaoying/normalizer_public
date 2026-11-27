import normalizer.intrinsics.N;

class BillMemoRow {
    int accountId;
    @N.Nullable String memo;
}
class NullMemoService {
    static int missingMemoCount(int accountId) {
        BillMemoRow[] bills;
        /* @normalizer-ir
           bills = from bill as b
               where b.account_id == accountId
               yield { accountId: b.account_id, memo: b.memo }
        */
        int count;
        /* @normalizer-ir
           count = count from bills as row where row.memo IS NULL
        */
        return count != -1 ? 7 : 11;
    }
}
