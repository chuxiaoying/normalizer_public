import normalizer.intrinsics.N;

class MemoRow { @N.Nullable String displayMemo; }

class DistinctMemo {
    static int count(int accountId) {
        MemoRow[] rows;
        /* @normalizer-ir
           rows = from bill as b
               where b.account_id == accountId
               yield { displayMemo: b.memo }
        */
        int result;
        /* @normalizer-ir result = count distinct from rows as row yield row.displayMemo */
        return result;
    }
}
