extends NobodyWhoChat

var time_of_start = null
var time_of_first_token = null
var total_token_count = 0
var time_of_completion = null

func _ready() -> void:
	time_of_start = Time.get_ticks_msec()
	start_worker()
	say("Please say 'apple' 1000 times.")

func _on_response_updated(new_token: String) -> void:
	if time_of_first_token == null:
		time_of_first_token = Time.get_ticks_msec()
	total_token_count += 1
	print(new_token)


func _on_response_finished(response: String) -> void:
	time_of_completion = Time.get_ticks_msec()
	var tokens_per_second = total_token_count / ((time_of_completion-time_of_first_token) / 1000)
	print("time to first token: " + str(time_of_first_token - time_of_start))
	print("tokens per second: " + str(tokens_per_second))
	print("total tokens count: " + str(total_token_count))
	
