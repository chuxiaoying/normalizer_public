class SwitchMode {
    static int classify(int code) {
        switch (code) {
            case 1: if (code == 1) return 99;
            case 2: throw new IllegalArgumentException();
            default: return 0;
        }
    }
}
