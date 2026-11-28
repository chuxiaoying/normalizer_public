class SwitchMode {
    static int classify(int code) {
        switch (code) {
            case 2:
                return 20;
            case 1:
                return 10;
            default:
                return 0;
        }
    }
}
