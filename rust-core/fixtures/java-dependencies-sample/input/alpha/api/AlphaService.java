package alpha.api;

import alpha.internal.AlphaHelper;
import beta.api.BetaService;
import beta.internal.BetaHelper;
import static beta.api.BetaService.makeValue;

public class AlphaService {
    public int total() {
        AlphaHelper helper = new AlphaHelper();
        return helper.value() + makeValue() + new BetaService().base() + new BetaHelper().delta();
    }
}
