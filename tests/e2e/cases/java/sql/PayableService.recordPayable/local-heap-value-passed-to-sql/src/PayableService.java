class DatabaseWriteError extends RuntimeException {}

class AmountBox {
    int value;
    AmountBox(int amount) { this.value = amount; }
}
class PayableService {
    static int recordPayable(int accountId, int amount) throws DatabaseWriteError {
        if (amount <= 0) return 0;
        AmountBox box = new AmountBox(amount);
        int savedAmount = box.value;
        int affected;
        /* @normalizer-ir
           affected = insert into payable_log
               values { account_id: accountId, amount: savedAmount }
               returning affected_rows
               atomic
               throws DatabaseWriteError
        */
        return affected;
    }
}
