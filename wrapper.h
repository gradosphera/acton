void* create_emulator(const char *config, int verbosity);

typedef const char* (*ExtFunc)(const char*);

char *emulate_with_emulator(void* em, const char* libs, const char* account, const char* message, const char* params);

const char *transaction_emulator_register_extmethod(void *transaction_emulator, int id,
                                                                           ExtFunc callback);


typedef void (*WasmFsReadCallback)(int kind, char const* data, char** dest_contents, char** dest_error);

const char *tolk_compile(const char *config_json, WasmFsReadCallback callback);
