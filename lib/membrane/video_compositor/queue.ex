defmodule Membrane.VideoCompositor.Queue do
  @moduledoc """
  Defines input pads and compositor core contracts, that each
  implementation of a queue should meet.
  """

  alias Membrane.{Buffer, Pad, RawVideo}
  alias Membrane.VideoCompositor
  alias Membrane.VideoCompositor.{CompositorCoreFormat, SceneChangeEvent}
  alias Membrane.VideoCompositor.Queue.Offline, as: OfflineQueue

  @typedoc """
  Defines stream format action send to VC Core by Queue.
  """
  @type stream_format_action :: {:stream_format, {Pad.ref_t(), CompositorCoreFormat.t()}}

  @typedoc """
  Defines scene update event action send to VC Core by Queue.
  """
  @type compositor_scene_event_action :: {:event, {:output, SceneChangeEvent.t()}}

  @typedoc """
  Defines frames buffer send to VC Core by Queue.
  """
  @type buffer :: %Buffer{payload: %{Pad.ref_t() => frame_data :: binary()}}

  @typedoc """
  Defines buffer action send to VC Core by Queue.
  """
  @type buffer_action :: {:buffer, {Pad.ref_t(), buffer()}}

  @typedoc """
  Defines actions send to VC Core by Queue.
  Stream format and scene event should be send before first buffer.
  """
  @type compositor_actions :: [
          stream_format_action()
          | compositor_scene_event_action()
          | buffer_action()
        ]

  @type queue_bin :: OfflineQueue.t()

  @spec get_queue(VideoCompositor.init_options()) :: OfflineQueue.t()
  def get_queue(%VideoCompositor{
        queuing_strategy: queuing_strategy,
        output_stream_format: %RawVideo{framerate: framerate}
      }) do
    case queuing_strategy do
      :offline -> %OfflineQueue{output_framerate: framerate}
    end
  end
end