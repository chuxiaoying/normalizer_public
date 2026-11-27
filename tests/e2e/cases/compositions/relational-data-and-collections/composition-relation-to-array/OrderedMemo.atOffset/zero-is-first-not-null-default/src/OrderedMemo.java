import normalizer.intrinsics.N;

class MemoSlot {
    int id;
    int priority;
    @N.Nullable String memo;
}

class OrderedMemo {
    @N.Nullable
    static String atOffset(int accountId) {
        MemoSlot[] rows;
        /* @normalizer-ir
           rows = from bill as b
               where b.account_id == accountId
               order by b.priority, b.id
               yield { id: b.id, priority: b.priority, memo: b.memo }
        */
        return rows[0].memo;
    }
}
