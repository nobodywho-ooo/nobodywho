extends NobodyWhoChat

var time_of_start = null
var time_of_first_token = null
var total_token_count = 0
var time_of_completion = null

func _ready() -> void:
	time_of_start = Time.get_ticks_msec()
	start_worker()
	say("Please say 'apple' 100 times.")

func _on_response_updated(new_token: String) -> void:
	if time_of_first_token == null:
		time_of_first_token = Time.get_ticks_msec()
	total_token_count += 1
	print(new_token)


func _on_response_finished(response: String) -> void:
	time_of_completion = Time.get_ticks_msec()
	print("total tokens count: " + str(total_token_count))
	print("time of start: " + str(time_of_start))
	print("time of first token: " + str(time_of_first_token))
	print("time to first token: " + str(time_of_first_token - time_of_start))
	print("time of completion: " + str(time_of_completion))
	var total_completion_time = time_of_completion - time_of_first_token
	print("total_completion_time_ms: " + str(total_completion_time))
	var total_completion_time_seconds = total_completion_time / 1000.0
	print("total_completion_time_s: " + str(total_completion_time_seconds)) 
	var tokens_per_second = total_token_count / total_completion_time_seconds
	print("tokens per second: " + str(tokens_per_second))
	
