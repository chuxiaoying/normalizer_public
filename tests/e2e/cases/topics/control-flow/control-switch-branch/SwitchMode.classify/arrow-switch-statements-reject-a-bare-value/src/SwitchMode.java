class SwitchMode {
    static int classify(int code) {
        switch (code) {
            case 1 -> 10;
            default -> { return 0; }
        }
        return 0;
    }
}
