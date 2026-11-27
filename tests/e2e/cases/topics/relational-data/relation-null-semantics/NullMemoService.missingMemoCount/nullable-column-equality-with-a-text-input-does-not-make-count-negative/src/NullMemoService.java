import normalizer.intrinsics.N;

class BillMemoRow {
    int accountId;
    @N.Nullable String memo;
}
class NullMemoService {
    static int missingMemoCount(int accountId, java.lang.String requestedMemo) {
        BillMemoRow[] bills;
        /* @normalizer-ir
           bills = from bill as b
               where b.account_id == accountId
               yield { accountId: b.account_id, memo: b.memo }
        */
        int count;
        /* @normalizer-ir
           count = count from bills as row where row.memo == requestedMemo
        */
        return count >= 0 ? 7 : 11;
    }
}
