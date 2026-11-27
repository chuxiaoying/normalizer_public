class UnsupportedBoundary {
    static String format(int value) {
        return runtimeFormat(value + 1);
    }

    static native String runtimeFormat(int value);
}
