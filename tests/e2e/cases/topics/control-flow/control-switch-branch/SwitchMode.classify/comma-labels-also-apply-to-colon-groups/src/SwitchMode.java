class SwitchMode {
    static int classify(int code) {
        switch (code) {
            case 1, 3: return 7;
            default: return 0;
        }
    }
}
