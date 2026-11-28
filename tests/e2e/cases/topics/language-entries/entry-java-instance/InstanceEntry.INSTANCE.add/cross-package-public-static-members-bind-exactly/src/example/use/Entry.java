package example.use;

import example.first.Tools;

public class Entry {
    public static int run(int units) {
        return Tools.change(units) * 10 + example.second.Tools.change(units);
    }
}
