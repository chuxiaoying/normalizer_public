class Entry {
    static String read(Object value, boolean stop) {
        if (!(value instanceof String text)) {
            if (stop) return "fallback";
        }
        return text;
    }
}
