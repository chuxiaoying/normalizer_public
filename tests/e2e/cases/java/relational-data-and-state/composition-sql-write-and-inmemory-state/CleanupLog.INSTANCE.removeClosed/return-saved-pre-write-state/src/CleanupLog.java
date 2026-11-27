import normalizer.intrinsics.N;

class DatabaseWriteError extends RuntimeException {}
class CleanupLog {
    static final CleanupLog INSTANCE = new CleanupLog();
    @N.EntryState int removed;

    int removeClosed(int accountId) {
        int before = this.removed;
        int affected;
        /* @normalizer-ir
           affected = delete from bill as b
               where b.account_id == accountId and b.status == "CLOSED"
               returning affected_rows
               atomic throws DatabaseWriteError
        */
        this.removed += affected;
        return before;
    }
}
