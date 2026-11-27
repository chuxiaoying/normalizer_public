import normalizer.intrinsics.N;

class Ticket { int id; }
class Row { @N.Nullable Ticket ticket; }
class Batch {
    static int firstId(Row[] rows, Row extra) {
        int[] ids;
        /* @normalizer-ir
           ids = from rows as row yield row.ticket.id
        */
        return extra.ticket.id;
    }
}
