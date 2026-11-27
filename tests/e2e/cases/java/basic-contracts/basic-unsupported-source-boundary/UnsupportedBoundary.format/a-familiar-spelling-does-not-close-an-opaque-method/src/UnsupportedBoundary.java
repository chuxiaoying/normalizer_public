class UnsupportedBoundary {
    static String format(int value) {
        return toString(value);
    }

    static native String toString(int value);
}
