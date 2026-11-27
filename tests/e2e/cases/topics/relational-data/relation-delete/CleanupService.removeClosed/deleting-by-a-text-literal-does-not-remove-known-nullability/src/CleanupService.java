import normalizer.intrinsics.N;
class DatabaseWriteError extends RuntimeException {}
class ObservedStatus { @N.Nullable String value; }

class CleanupService {
    static int removeClosed(int accountId) {
        ObservedStatus[] observed; /* @normalizer-ir observed = from bill as r yield {value: r.status} */ int affected;
        /* @normalizer-ir
           affected = delete from bill as b
               where b.account_id == accountId and b.status == "CLOSED"
               returning affected_rows
               atomic throws DatabaseWriteError
        */
        return affected;
    }
}
