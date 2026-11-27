class SwitchMode {
    static int classify(int code) {
        switch (code) {
            case 1:
                return 10;
            case 2:
                return 30;
            default:
                return 0;
        }
    }
}
