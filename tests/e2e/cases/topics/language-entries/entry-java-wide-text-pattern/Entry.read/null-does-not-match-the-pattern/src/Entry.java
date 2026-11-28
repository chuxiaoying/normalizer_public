class Entry {
    static String read() {
        Object value = null;
        if (value instanceof String text) return text;
        return "fallback";
    }
}
