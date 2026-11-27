class TicketCode {
    static int readValid(int value) { synchronized (TicketCode.class) { return value; } }
    static native int booleanValue();
}
