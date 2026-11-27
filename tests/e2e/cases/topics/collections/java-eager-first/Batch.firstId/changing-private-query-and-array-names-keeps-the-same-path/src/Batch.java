import normalizer.intrinsics.N;

class Ticket { int id; }
class Row { @N.Nullable Ticket ticket; }
class Batch {
    static int firstId(Row[] rows) {
        int[] projected;
        /* @normalizer-ir
           projected = from rows as candidate yield candidate.ticket.id
        */
        return projected[0];
    }
}
