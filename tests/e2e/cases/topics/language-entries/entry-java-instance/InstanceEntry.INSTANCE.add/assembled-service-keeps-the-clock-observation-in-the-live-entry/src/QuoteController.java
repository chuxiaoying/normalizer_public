import normalizer.intrinsics.N;
class PriceService {
    long quote(int units) { return N.currentTimeMillis() + units; }
}
class QuoteController {
    private PriceService price;
    void setPrice(PriceService price) { this.price = price; }
    public long quote(int units) { return price.quote(units); }
}
