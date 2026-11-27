class DatabaseWriteError extends RuntimeException {}

class WaiverService {
    static int setOpenBillWaiver(int accountId, int waivedAmount) throws DatabaseWriteError {
        int affected;
        /* @normalizer-ir
           affected = update bill as b
               set { waived_amount: waivedAmount }
               where b.account_id == accountId and b.status == "CLOSED"
               returning affected_rows
               atomic
               throws DatabaseWriteError
        */
        return affected;
    }
}
