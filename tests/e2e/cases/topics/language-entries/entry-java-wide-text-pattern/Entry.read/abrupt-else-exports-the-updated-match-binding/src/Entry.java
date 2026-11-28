class Entry {
    static String read(Object value) {
        if (value instanceof String text) {
            text = "fixed";
        } else throw new IllegalArgumentException("guard");
        return text;
    }
}
