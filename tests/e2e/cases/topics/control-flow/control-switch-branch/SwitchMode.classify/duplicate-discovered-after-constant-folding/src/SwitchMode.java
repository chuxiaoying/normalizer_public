class SwitchMode {
    static int classify(int code) {
        switch (code) {
            case 1 + 2 -> { return 10; }
            case 3 -> { return 20; }
            default -> { return 0; }
        }
    }
}
