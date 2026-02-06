import os

import lm_eval
import nobodywho
from tqdm import tqdm


@register_model
class NobodyWhoLM(lm_eval.api.model.LM):
    chat: nobodywho.Chat

    def __init__(self, *args, **kwargs):
        model_path = os.getenv("TEST_MODEL")
        assert isinstance(model_path, str)
        self.chat = nobodywho.Chat(model_path)

    def generate_until(
        self, requests: list[lm_eval.api.instance.Instance], disable_tqdm=False
    ):
        result: list[str] = []
        for request in tqdm([req.args for req in requests], disable=disable_tqdm):
            self.chat.reset_history()
            text = request[0]
            assert isinstance(text, str)

            # XXX: these provide additional generation args like stopwords or max_tokens
            request_args = request[1]

            response_text = self.chat.ask(text).completed()
            result.append(response_text)
        return result

    def loglikelihood(self, *args, **kwargs):
        raise NotImplementedError

    def loglikelihood_rolling(self, *args, **kwargs):
        raise NotImplementedError


if __name__ == "__main__":
    pass
