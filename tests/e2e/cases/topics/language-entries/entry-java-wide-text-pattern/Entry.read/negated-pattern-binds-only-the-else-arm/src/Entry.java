class Entry {
    static String read(Object value) {
        if (!(value instanceof String text)) return "fallback";
        else return text;
    }
}
