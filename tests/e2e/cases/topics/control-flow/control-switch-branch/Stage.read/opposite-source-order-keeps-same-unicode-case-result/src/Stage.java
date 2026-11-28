class Stage {
    static int read(String mode) {
        switch (mode) {
            case "\uD800":
                return 1;
            case "\uD801":
                return 2;
            case "\uE000":
                return 3;
            case "\uD800\uDC00":
                return 4;
            default:
                return 0;
        }
    }
}
