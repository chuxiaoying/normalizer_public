class Stage {
    static int read(String mode) {
        switch (mode) {
            case "\uD800\uDC00":
                return 4;
            case "\uE000":
                return 3;
            case "\uD801":
                return 2;
            case "\uD800":
                return 1;
            default:
                return 0;
        }
    }
}
