class SwitchMode {
    static int classify(int code) {
        int x = 2;
        switch (x--) { case 1 -> x = 99; default -> {} } return x;
    }
}
