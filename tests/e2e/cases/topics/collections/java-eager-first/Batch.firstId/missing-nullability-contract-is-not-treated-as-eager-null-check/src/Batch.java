import normalizer.intrinsics.N;

class Ticket { int id; }
class Row { Ticket ticket; }
class Batch {
    static int firstId(Row[] rows) {
        int[] ids;
        /* @normalizer-ir
           ids = from rows as row yield row.ticket.id
        */
        return ids[0];
    }
}
