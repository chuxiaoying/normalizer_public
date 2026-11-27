import normalizer.intrinsics.N;

class Ticket { int id; }
class Row { @N.Nullable Ticket ticket; }
class Batch {
    static int firstId(Row[] rows) {
        Row[] same = rows; int[] ids;
        /* @normalizer-ir
           ids = from same as row yield row.ticket.id
        */
        return ids[0];
    }
}
