class Money {
    static int toMinorUnits(int major, int minor) {
        return major * 100 - minor;
    }
}
