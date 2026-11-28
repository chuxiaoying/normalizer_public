class Stage {
    static int read(String mode) {
        switch (mode) {
            case "ready":
                return 1;
            case "re\u0061dy":
                return 2;
            default:
                return 0;
        }
    }
}
