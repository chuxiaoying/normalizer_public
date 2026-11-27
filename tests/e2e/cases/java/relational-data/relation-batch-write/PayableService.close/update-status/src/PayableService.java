class DatabaseWriteError extends RuntimeException {}

class PayableService {
    static int close(int accountId, int amount) throws DatabaseWriteError {
        if (amount > 0) {
            int affected;
            /* @normalizer-ir
               affected = update payable as p
                   set { closed_amount: amount, status: "SETTLED" }
                   where p.account_id == accountId
                   returning affected_rows
                   atomic
                   throws DatabaseWriteError
            */
        }
        return amount;
    }
}
