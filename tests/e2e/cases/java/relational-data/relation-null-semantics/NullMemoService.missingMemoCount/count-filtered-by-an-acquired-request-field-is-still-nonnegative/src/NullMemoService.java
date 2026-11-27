import normalizer.intrinsics.N;

class BillMemoRow {
    int accountId;
    @N.Nullable String memo;
}
class MemoFilter { int accountId; }
class NullMemoService {
    static int missingMemoCount(MemoFilter request) {
        int accountId = request.accountId;
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
        return count < 0 ? 11 : 7;
    }
}
