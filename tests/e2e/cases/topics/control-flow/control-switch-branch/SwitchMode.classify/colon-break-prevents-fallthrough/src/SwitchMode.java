class SwitchMode {
    static int classify(int code) {
        int result = 0;
        switch (code) {
            case 1: result = 10; break;
            case 2: result += 5; break;
            default: result = 1;
        }
        return result;
    }
}
