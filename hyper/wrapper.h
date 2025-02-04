#include <stdint.h>
#include <stdlib.h>

// Initialize the HyperSDK runtime with the given configuration
int hypersdk_init_runtime(const char* config);

// Deploy a new contract and return its ID
int hypersdk_deploy_contract(const uint8_t* wasm_bytes, size_t wasm_len, uint8_t contract_id[32]);

// Call a contract function
int hypersdk_call_contract(
    const uint8_t contract_id[32],
    const char* function,
    const uint8_t* args,
    size_t args_len,
    uint8_t** result,
    size_t* result_len
);

// Get contract state
int hypersdk_get_state(
    const uint8_t contract_id[32],
    uint8_t** state,
    size_t* state_len
);

// Free a buffer allocated by the Go runtime
void hypersdk_free_buffer(void* ptr);
