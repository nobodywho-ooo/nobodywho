#include <algorithm>
#include <chrono>
#include <cstdlib>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include "llama.h"

static std::vector<llama_token> read_tokens(const char * path) {
    std::ifstream input(path);
    std::string json((std::istreambuf_iterator<char>(input)), std::istreambuf_iterator<char>());
    std::vector<llama_token> tokens;
    for (size_t index = 0; index < json.size();) {
        if (json[index] < '0' || json[index] > '9') {
            ++index;
            continue;
        }
        size_t end = index;
        while (end < json.size() && json[end] >= '0' && json[end] <= '9') {
            ++end;
        }
        tokens.push_back(std::stoi(json.substr(index, end - index)));
        index = end;
    }
    return tokens;
}

static std::string detokenize(const llama_vocab * vocab, const std::vector<llama_token> & tokens) {
    std::string text(tokens.size() * 16 + 256, '\0');
    int32_t length = llama_detokenize(
        vocab,
        tokens.data(),
        static_cast<int32_t>(tokens.size()),
        text.data(),
        static_cast<int32_t>(text.size()),
        true,
        false);
    if (length < 0) {
        text.resize(static_cast<size_t>(-length));
        length = llama_detokenize(
            vocab,
            tokens.data(),
            static_cast<int32_t>(tokens.size()),
            text.data(),
            static_cast<int32_t>(text.size()),
            true,
            false);
    }
    if (length < 0) {
        return "<detokenization failed>";
    }
    text.resize(static_cast<size_t>(length));
    return text;
}

static void print_json_string(const std::string & value) {
    std::cout << '"';
    for (const unsigned char byte : value) {
        switch (byte) {
            case '"': std::cout << "\\\""; break;
            case '\\': std::cout << "\\\\"; break;
            case '\b': std::cout << "\\b"; break;
            case '\f': std::cout << "\\f"; break;
            case '\n': std::cout << "\\n"; break;
            case '\r': std::cout << "\\r"; break;
            case '\t': std::cout << "\\t"; break;
            default:
                if (byte < 0x20) {
                    std::cout << "\\u" << std::hex << std::setw(4) << std::setfill('0')
                              << static_cast<int>(byte) << std::dec << std::setfill(' ');
                } else {
                    std::cout << byte;
                }
        }
    }
    std::cout << '"';
}

static void print_tokens(const std::vector<llama_token> & tokens) {
    std::cout << '[';
    for (size_t index = 0; index < tokens.size(); ++index) {
        if (index > 0) {
            std::cout << ',';
        }
        std::cout << tokens[index];
    }
    std::cout << ']';
}

int main(int argc, char ** argv) {
    if (argc != 5 && argc != 7) {
        std::cerr << "usage: llama-greedy MODEL TOKEN_COUNT CONTEXT_SIZE FLASH_ATTN [DIRECT_TOKENS PROMPT_TOKENS]\n";
        return 2;
    }

    const int token_count = std::atoi(argv[2]);
    const int context_size = std::atoi(argv[3]);
    const bool flash_attention = std::string(argv[4]) == "on";
    if (token_count <= 0 || context_size < token_count) {
        std::cerr << "invalid token or context count\n";
        return 2;
    }

    llama_backend_init();
    llama_model_params model_params = llama_model_default_params();
    model_params.n_gpu_layers = 99;
    llama_model * model = llama_model_load_from_file(argv[1], model_params);
    if (model == nullptr) {
        std::cerr << "failed to load model\n";
        llama_backend_free();
        return 1;
    }

    llama_context_params context_params = llama_context_default_params();
    context_params.n_ctx = context_size;
    context_params.n_batch = 1;
    context_params.n_ubatch = 1;
    context_params.n_threads = 1;
    context_params.n_threads_batch = 1;
    context_params.type_k = GGML_TYPE_F16;
    context_params.type_v = GGML_TYPE_F16;
    context_params.flash_attn_type = flash_attention
        ? LLAMA_FLASH_ATTN_TYPE_ENABLED
        : LLAMA_FLASH_ATTN_TYPE_DISABLED;
    llama_context * context = llama_init_from_model(model, context_params);
    if (context == nullptr) {
        std::cerr << "failed to create context\n";
        llama_model_free(model);
        llama_backend_free();
        return 1;
    }

    const llama_vocab * vocab = llama_model_get_vocab(model);
    const int vocabulary_size = llama_vocab_n_tokens(vocab);
    const std::vector<llama_token> prompt = argc == 7
        ? read_tokens(argv[6])
        : std::vector<llama_token>{2};
    if (prompt.empty()) {
        std::cerr << "prompt has no tokens\n";
        llama_free(context);
        llama_model_free(model);
        llama_backend_free();
        return 1;
    }
    llama_token warmup_token = prompt[0];
    if (llama_decode(context, llama_batch_get_one(&warmup_token, 1)) != 0) {
        std::cerr << "warmup decode failed\n";
        llama_free(context);
        llama_model_free(model);
        llama_backend_free();
        return 1;
    }
    llama_synchronize(context);
    llama_memory_clear(llama_get_memory(context), true);
    llama_synchronize(context);

    const auto started = std::chrono::steady_clock::now();
    for (size_t position = 0; position < prompt.size(); ++position) {
        llama_token prompt_token = prompt[position];
        if (llama_decode(context, llama_batch_get_one(&prompt_token, 1)) != 0) {
            std::cerr << "prompt decode failed at position " << position << '\n';
            llama_free(context);
            llama_model_free(model);
            llama_backend_free();
            return 1;
        }
        llama_synchronize(context);
    }
    const float * logits = llama_get_logits_ith(context, -1);
    llama_token token = std::max_element(logits, logits + vocabulary_size) - logits;
    std::vector<llama_token> tokens;
    tokens.reserve(static_cast<size_t>(token_count));
    for (int index = 0; index < token_count; ++index) {
        tokens.push_back(token);
        if (argc == 7 && (token == 1 || token == 106)) {
            break;
        }
        if (index + 1 < token_count) {
            if (llama_decode(context, llama_batch_get_one(&token, 1)) != 0) {
                std::cerr << "completion decode failed at position " << index << '\n';
                llama_free(context);
                llama_model_free(model);
                llama_backend_free();
                return 1;
            }
            llama_synchronize(context);
            logits = llama_get_logits_ith(context, -1);
            token = std::max_element(logits, logits + vocabulary_size) - logits;
        }
    }
    const auto elapsed = std::chrono::steady_clock::now() - started;
    const double latency_ms = std::chrono::duration<double, std::milli>(elapsed).count();
    const double tokens_per_second = static_cast<double>(tokens.size()) * 1000.0 / latency_ms;

    if (argc == 5) {
        print_tokens(tokens);
        std::cout << '\n';
    } else {
        const std::vector<llama_token> direct_tokens = read_tokens(argv[5]);
        std::cout << "{\n  \"tokens\": ";
        print_tokens(tokens);
        std::cout << ",\n  \"completion\": ";
        print_json_string(detokenize(vocab, tokens));
        std::cout << ",\n  \"direct_completion\": ";
        print_json_string(detokenize(vocab, direct_tokens));
        std::cout << ",\n  \"latency_ms\": " << std::fixed << std::setprecision(3) << latency_ms
                  << ",\n  \"tokens_per_second\": " << std::setprecision(3) << tokens_per_second
                  << "\n}\n";
    }

    llama_free(context);
    llama_model_free(model);
    llama_backend_free();
    return 0;
}
