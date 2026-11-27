class DatabaseWriteError extends RuntimeException {}

class PayableService {
    static int recordPayable(int accountId, int amount) throws DatabaseWriteError {
        if (!(amount > 0)) return 0;
        int affected;
        /* @normalizer-ir
           affected = insert into payable_log
               values { account_id: accountId, amount: amount }
               returning affected_rows
               atomic
               throws DatabaseWriteError
        */
        return affected;
    }
}
