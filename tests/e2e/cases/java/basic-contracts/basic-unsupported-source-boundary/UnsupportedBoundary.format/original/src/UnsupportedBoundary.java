class UnsupportedBoundary {
    static String format(int value) {
        return runtimeFormat(value);
    }

    static native String runtimeFormat(int value);
}
