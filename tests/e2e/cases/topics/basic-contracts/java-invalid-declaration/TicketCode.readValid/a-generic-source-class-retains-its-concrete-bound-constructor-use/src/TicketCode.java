class Box<T> { final T x; Box(T x) { this.x = x; } } class TicketCode {
    static int readValid(int value) { Box<String> x = new Box<String>("ok"); return x.x == null ? 0 : value; }
    static native int booleanValue();
}
