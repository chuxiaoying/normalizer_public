class Pair<X,Y> { final Y second; final X first; Pair(X x, Y y) { this.second = y; this.first = x; } } class TicketCode {
    static String readValid(int value) { Pair<String,java.lang.String> pair = new Pair<String,java.lang.String>("LEFT", "right"); String first = pair.first; java.lang.String second = pair.second; return first + second; }
    static native int booleanValue();
}
