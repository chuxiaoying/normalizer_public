class DatabaseWriteError extends RuntimeException {}

class CleanupService {
    static int removeClosed(int accountId) {
        int shiftedAccount = accountId + 1; int affected;
        /* @normalizer-ir
           affected = delete from bill as b
               where b.account_id == shiftedAccount and b.status == "CLOSED"
               returning affected_rows
               atomic throws DatabaseWriteError
        */
        return affected;
    }
}
