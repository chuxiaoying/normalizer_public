class SwitchMode {
    static int classify(int code) {
        switch (code) {
            case 1: return 10;
            default -> { return 0; }
        }
    }
}
