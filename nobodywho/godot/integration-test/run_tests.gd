extends Control


func _ready() -> void:
	print("👷 running tests...")
	$NobodyWhoChat.set_log_level("info")
	assert(await $NobodyWhoEmbedding.run_test())
	assert(await $NobodyWhoChat.run_test())
	assert(await $Grammar.run_test())
	assert(await $CrossEncoder.run_test())
	print("✨ all tests complete")
	get_tree().quit()
