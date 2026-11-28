class PriceService {
    int quote(int units) { return units * 3; }
}
class QuoteController {
    private PriceService price;
    void inject(PriceService price) { this.price = price; }
    public int quote(int units) { return price.quote(units); }
}
