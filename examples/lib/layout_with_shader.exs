defmodule LayoutWithShaderPipeline do
  @moduledoc false

  use Membrane.Pipeline

  require Membrane.Logger

  alias Membrane.LiveCompositor
  alias Membrane.PortAudio

  @output_width 1920
  @output_height 1080
  @video_output_id "video_output"
  @audio_output_id "audio_output"
  @shader_id "example_shader"
  @shader_path "./lib/example_shader.wgsl"

  @impl true
  def handle_init(_ctx, %{
        server_setup: server_setup,
        video_sample_path: video_sample_path,
        audio_sample_path: audio_sample_path
      }) do
    spec = [
      child({:video_src, 0}, %Membrane.File.Source{location: video_sample_path})
      |> child({:input_parser, 0}, %Membrane.H264.Parser{
        output_alignment: :nalu,
        generate_best_effort_timestamps: %{framerate: {30, 1}}
      })
      |> child({:realtimer, 0}, Membrane.Realtimer)
      |> via_in(Pad.ref(:video_input, "video_input_0"))
      |> child(:video_compositor, %Membrane.LiveCompositor{
        framerate: {30, 1},
        server_setup: server_setup,
        init_requests: [
          register_shader_request_body()
        ]
      })
      |> via_out(Pad.ref(:video_output, @video_output_id),
        options: [
          width: @output_width,
          height: @output_height,
          initial:
            scene([
              "video_input_0"
            ])
        ]
      )
      |> child(:output_parser, Membrane.H264.Parser)
      |> child(:output_decoder, Membrane.H264.FFmpeg.Decoder)
      |> child(:sdl_player, Membrane.SDL.Player),
      child({:audio_src, 0}, %Membrane.File.Source{location: audio_sample_path})
      |> child({:audio_demuxer, 0}, Membrane.Ogg.Demuxer),
      get_child(:video_compositor)
      |> via_out(Pad.ref(:audio_output, @audio_output_id),
        options: [
          channels: :stereo,
          initial: %{
            inputs: [
              %{input_id: "audio_input_0"}
            ]
          }
        ]
      )
      |> child(:audio_output_parser, Membrane.Opus.Parser)
      |> child(:audio_output_decoder, Membrane.Opus.Decoder)
      |> child(:pa_sink, PortAudio.Sink)
    ]

    {[spec: spec],
     %{
       videos_count: 1,
       video_sample_path: video_sample_path,
       audio_sample_path: audio_sample_path
     }}
  end

  @impl true
  def handle_setup(_ctx, state) do
    {[start_timer: {:add_videos_timer, Membrane.Time.seconds(3)}], state}
  end

  @impl true
  def handle_child_notification(
        {:input_registered, Pad.ref(:video_input, _input_id), ctx},
        :video_compositor,
        _ctx,
        state
      ) do
    update_scene_request = %{
      type: :update_output,
      output_id: @video_output_id,
      video: scene(ctx.video_inputs)
    }

    {[{:notify_child, {:video_compositor, {:lc_request, update_scene_request}}}], state}
  end

  @impl true
  def handle_child_notification(
        {:input_registered, Pad.ref(:audio_input, _input_id), ctx},
        :video_compositor,
        _ctx,
        state
      ) do
    update_audio_request = %{
      type: :update_output,
      output_id: @audio_output_id,
      audio: %{
        inputs: ctx.audio_inputs |> Enum.map(fn input_id -> %{input_id: input_id} end)
      }
    }

    {[{:notify_child, {:video_compositor, {:lc_request, update_audio_request}}}], state}
  end

  @impl true
  def handle_child_notification(
        {:lc_request_response, req, %Req.Response{status: response_code, body: response_body},
         _lc_ctx},
        _child,
        _membrane_ctx,
        state
      ) do
    if response_code != 200 do
      raise """
      Request failed.
      Request: `#{inspect(req)}.
      Response code: #{response_code}.
      Response body: #{inspect(response_body)}.
      """
    end

    {[], state}
  end

  @impl true
  def handle_child_notification(
        {:new_track, {track_id, _track_type}},
        {:audio_demuxer, id},
        _membrane_ctx,
        state
      ) do
    spec =
      get_child({:audio_demuxer, id})
      |> via_out(Pad.ref(:output, track_id))
      |> child({:audio_input_parser, id}, %Membrane.Opus.Parser{
        generate_best_effort_timestamps?: true
      })
      |> child({:realtimer_audio, id}, Membrane.Realtimer)
      |> via_in(Pad.ref(:audio_input, "audio_input_#{id}"))
      |> get_child(:video_compositor)

    {[spec: spec], state}
  end

  @impl true
  def handle_child_notification(notification, child, _ctx, state) do
    Membrane.Logger.debug(
      "Received notification: #{inspect(notification)} from child: #{inspect(child)}."
    )

    {[], state}
  end

  @impl true
  def handle_tick(:add_videos_timer, _ctx, state) do
    videos_count = state.videos_count

    if state.videos_count < 10 do
      spec = [
        child({:video_src, videos_count}, %Membrane.File.Source{location: state.video_sample_path})
        |> child({:input_parser, videos_count}, %Membrane.H264.Parser{
          output_alignment: :nalu,
          generate_best_effort_timestamps: %{framerate: {30, 1}}
        })
        |> child({:realtimer, videos_count}, Membrane.Realtimer)
        |> via_in(Pad.ref(:video_input, "video_input_#{videos_count}"))
        |> get_child(:video_compositor),
        child({:audio_src, videos_count}, %Membrane.File.Source{location: state.audio_sample_path})
        |> child({:audio_demuxer, videos_count}, Membrane.Ogg.Demuxer)
      ]

      {[spec: spec], %{state | videos_count: state.videos_count + 1}}
    else
      {[stop_timer: :add_videos_timer], state}
    end
  end

  @spec scene(list(LiveCompositor.input_id())) :: map()
  defp scene(input_ids) do
    %{
      type: :shader,
      shader_id: @shader_id,
      resolution: %{
        width: @output_width,
        height: @output_height
      },
      children: [
        %{
          id: "tile_0",
          type: :tiles,
          width: @output_width,
          height: @output_height,
          background_color_rgba: "#000088FF",
          transition: %{
            duration_ms: 300
          },
          margin: 10,
          children:
            input_ids |> Enum.map(fn input_id -> %{type: :input_stream, input_id: input_id} end)
        }
      ]
    }
  end

  defp register_shader_request_body() do
    %{
      type: :register,
      entity_type: :shader,
      shader_id: @shader_id,
      source: File.read!(@shader_path)
    }
  end
end

Utils.FFmpeg.generate_sample_video()

server_setup = Utils.LcServer.server_setup({30, 1})

{:ok, _supervisor, _pid} =
  Membrane.Pipeline.start_link(LayoutWithShaderPipeline, %{
    video_sample_path: "samples/testsrc.h264",
    audio_sample_path: "samples/test.ogg",
    server_setup: server_setup
  })

Process.sleep(:infinity)
